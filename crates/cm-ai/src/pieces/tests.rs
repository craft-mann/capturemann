use std::sync::Mutex;

use cm_core::{CmError, MediaTime};

use super::*;
use crate::transcribe::{cancelled, Transcript, TRANSCRIPT_VERSION};

const RATE: usize = SAMPLE_RATE as usize;

fn word(text: &str, at: f64) -> Word {
    Word {
        text: text.to_owned(),
        start: media_secs(at),
        end: media_secs(at + 0.3),
        confidence: 0.9,
    }
}

fn words_of(text: &str) -> Vec<Word> {
    text.split_whitespace()
        .enumerate()
        .map(|(i, w)| word(w, i as f64 * 0.4))
        .collect()
}

/// `secs` seconds of a steady tone, with a quiet stretch of `quiet_len` seconds at each `quiet_at`.
fn audio(secs: f64, quiet: &[(f64, f64)]) -> Vec<f32> {
    let mut x: Vec<f32> = (0..(secs * RATE as f64) as usize)
        .map(|i| 0.3 * (i as f32 * 0.05).sin())
        .collect();
    for (at, len) in quiet {
        let n = x.len();
        let (a, b) = (
            (at * RATE as f64) as usize,
            ((at + len) * RATE as f64) as usize,
        );
        for v in &mut x[a..b.min(n)] {
            *v *= 0.001;
        }
    }
    x
}

#[test]
fn short_audio_is_one_piece() {
    let x = audio(590.0, &[]);
    assert_eq!(plan(&x), vec![0..x.len()]);
    assert_eq!(plan(&[]), vec![0..0]);
}

/// Long audio is cut at the quiet spot near each target, and the pieces tile the audio.
#[test]
fn long_audio_is_cut_in_quiet_spots_and_the_pieces_tile_it() {
    // 1,500 s, quiet for 1 s near 312 s and near 598 s (both within 20 s of a 300 s target).
    let x = audio(1500.0, &[(312.0, 1.0), (598.0, 1.0)]);
    let p = plan(&x);
    assert!(p.len() >= 4, "{p:?}");
    assert_eq!(p[0].start, 0);
    assert_eq!(p.last().unwrap().end, x.len());
    for w in p.windows(2) {
        assert_eq!(w[0].end, w[1].start, "no gap and no overlap");
    }
    let cut = |i: usize| p[i].end as f64 / RATE as f64;
    assert!((312.0..313.0).contains(&cut(0)), "first cut at {}", cut(0));
    assert!((598.0..599.0).contains(&cut(1)), "second cut at {}", cut(1));
    for r in &p {
        let s = r.len() as f64 / RATE as f64;
        assert!((150.0..=450.0 + 40.0).contains(&s), "a piece of {s} s");
    }
}

#[test]
fn runs_are_found_but_ordinary_speech_is_not_one() {
    // Natural talk, with a repeated "yeah yeah yeah" and a repeated phrase of three.
    let talk = words_of(
        "so we agreed yeah yeah yeah that the pricing sheet goes out on Friday and then \
         the pricing sheet goes out and the pricing sheet goes out again to the customer",
    );
    assert!(find_runs(&talk).is_empty(), "{:?}", find_runs(&talk));
    assert!(!is_degenerate(&talk));

    let looped = words_of(&"da tra lai cho minh dau roi ".repeat(30));
    let runs = find_runs(&looped);
    assert_eq!(runs.len(), 1);
    assert_eq!((runs[0].unit, runs[0].count), (7, 30));
    assert!(is_degenerate(&looped));

    // Case and punctuation do not hide a loop.
    let punctuated: Vec<Word> = (0..40)
        .flat_map(|i| {
            [
                word("Đã,", i as f64),
                word("trả", i as f64),
                word("lại?", i as f64),
            ]
        })
        .collect();
    assert!(is_degenerate(&punctuated));

    // One word said nine times in a row is a run; eight is, seven is not.
    assert_eq!(find_runs(&words_of("a b c ừ ừ ừ ừ ừ ừ ừ ừ d e f")).len(), 1);
    assert!(find_runs(&words_of("a b c ừ ừ ừ ừ ừ ừ ừ d e f")).is_empty());
}

/// A few real repeats inside a long, real transcript do not make the piece a loop.
#[test]
fn a_loop_has_to_be_a_large_part_of_the_piece() {
    let mut w = words_of("the quick brown fox jumps over a lazy dog today");
    let real: Vec<Word> = (0..300)
        .map(|i| word(&format!("word{i}"), i as f64))
        .collect();
    w.extend(real);
    w.extend(words_of(&"ok then ".repeat(5))); // a short run of 10 words in 310
    assert!(!find_runs(&w).is_empty());
    assert!(!is_degenerate(&w));
}

#[test]
fn collapsing_keeps_one_of_each_run_and_the_rest_in_order() {
    let mut w = words_of("start here");
    w.extend(words_of(&"da tra lai ".repeat(10)));
    w.extend(words_of("and the end"));
    let (kept, dropped) = collapse_repeats(w.clone());
    let text: Vec<&str> = kept.iter().map(|w| w.text.as_str()).collect();
    assert_eq!(
        text,
        ["start", "here", "da", "tra", "lai", "and", "the", "end"]
    );
    assert_eq!(dropped, w.len() - kept.len());
    // Nothing to collapse: the words come back untouched.
    let plain = words_of("one two three four");
    assert_eq!(collapse_repeats(plain.clone()), (plain, 0));
}

/// What the scripted engine was asked: the piece's seconds, the language and the prompt.
type Call = (f64, Option<String>, Option<String>);

/// A scripted engine: one word per 10 s of audio, except that a piece longer than `loop_over`
/// seconds comes back as a loop, like Large v3 did.
struct Engine {
    loop_over: f64,
    calls: Mutex<Vec<Call>>,
    language: &'static str,
    cancel_after: Option<usize>,
}

impl Engine {
    fn new(loop_over: f64) -> Engine {
        Engine {
            loop_over,
            calls: Mutex::new(Vec::new()),
            language: "vi",
            cancel_after: None,
        }
    }
}

impl Transcriber for Engine {
    fn transcribe(
        &self,
        audio: Pcm16k<'_>,
        language: Option<&str>,
        progress: &mut dyn FnMut(f32) -> bool,
    ) -> Result<Transcript, CmError> {
        self.transcribe_prompted(audio, language, None, progress)
    }

    fn transcribe_prompted(
        &self,
        audio: Pcm16k<'_>,
        language: Option<&str>,
        prompt: Option<&str>,
        progress: &mut dyn FnMut(f32) -> bool,
    ) -> Result<Transcript, CmError> {
        let secs = audio.samples.len() as f64 / RATE as f64;
        let n = {
            let mut c = self.calls.lock().unwrap();
            c.push((secs, language.map(str::to_owned), prompt.map(str::to_owned)));
            c.len()
        };
        if self.cancel_after.is_some_and(|k| n > k) {
            return Err(cancelled());
        }
        for i in 1..=4 {
            if !progress(i as f32 / 4.0) {
                return Err(cancelled());
            }
        }
        let start = audio.start.as_secs_f64();
        let words: Vec<Word> = if secs > self.loop_over {
            (0..(secs as usize).max(40))
                .map(|i| {
                    word(
                        ["da", "tra", "lai", "cho", "minh"][i % 5],
                        start + i as f64 * 0.2,
                    )
                })
                .collect()
        } else {
            (0..(secs / 10.0) as usize)
                .map(|i| {
                    word(
                        &format!("w{}", (start + i as f64 * 10.0) as u64),
                        start + i as f64 * 10.0,
                    )
                })
                .collect()
        };
        Ok(Transcript {
            version: TRANSCRIPT_VERSION,
            language: self.language.to_owned(),
            model: "scripted".into(),
            words,
            note: None,
        })
    }
}

fn run(
    e: &Engine,
    x: &[f32],
    language: Option<&str>,
    progress: &mut dyn FnMut(f32) -> bool,
) -> Result<(Transcript, PiecesReport), CmError> {
    let audio = Pcm16k {
        samples: x,
        start: MediaTime::from_ticks(1200),
    };
    transcribe_pieces(e, audio, language, Some("glossary: Linh, Minh"), progress)
}

#[test]
fn a_long_recording_is_transcribed_piece_by_piece_with_the_glossary_every_time() {
    let x = audio(1500.0, &[]);
    let e = Engine::new(1e9);
    let (t, report) = run(&e, &x, None, &mut |_| true).unwrap();
    let calls = e.calls.lock().unwrap();
    assert!(calls.len() >= 4, "{} pieces", calls.len());
    assert_eq!(report.pieces, calls.len());
    assert_eq!((report.split, report.repeats_dropped), (0, 0));
    // The glossary starts every piece, not only the first.
    assert!(calls
        .iter()
        .all(|c| c.2.as_deref() == Some("glossary: Linh, Minh")));
    // The language is detected by the first piece and fixed for the others.
    assert_eq!(calls[0].1, None);
    assert!(calls[1..].iter().all(|c| c.1.as_deref() == Some("vi")));
    assert_eq!(t.language, "vi");
    // Word times are in file time (the audio's start plus the position), in order.
    assert!(t.words.windows(2).all(|w| w[0].start <= w[1].start));
    assert!(t.words[0].start >= MediaTime::from_ticks(1200));
    let total: f64 = calls.iter().map(|c| c.0).sum();
    assert!(
        (total - 1500.0).abs() < 1.0,
        "the pieces cover the audio once: {total}"
    );
}

#[test]
fn a_language_that_was_asked_for_is_used_for_every_piece() {
    let x = audio(1500.0, &[]);
    let e = Engine::new(1e9);
    run(&e, &x, Some("en"), &mut |_| true).unwrap();
    assert!(e
        .calls
        .lock()
        .unwrap()
        .iter()
        .all(|c| c.1.as_deref() == Some("en")));
    let e = Engine::new(1e9);
    run(&e, &x, Some("auto"), &mut |_| true).unwrap();
    assert_eq!(
        e.calls.lock().unwrap()[0].1,
        None,
        "auto is detected, not passed on"
    );
}

/// The Oct 28, 2026 failure: pieces that loop are cut in half until they do not, and the result is
/// the real words, not a loop.
#[test]
fn a_piece_that_loops_is_cut_in_half_and_done_again() {
    let x = audio(1500.0, &[]);
    // Any piece over 100 s loops: 300 s pieces need two cuts (to about 75 s).
    let e = Engine::new(100.0);
    let (t, report) = run(&e, &x, None, &mut |_| true).unwrap();
    assert!(report.split >= 4, "{report:?}");
    assert_eq!(report.repeats_dropped, 0, "the cuts were enough");
    assert!(!is_degenerate(&t.words));
    assert!(find_runs(&t.words).is_empty(), "no loop is left");
    // Words from the whole timeline: something near the start and near the end.
    assert!(t.words.first().unwrap().start.as_secs_f64() < 60.0);
    assert!(t.words.last().unwrap().start.as_secs_f64() > 1400.0);
    assert!(t.words.windows(2).all(|w| w[0].start <= w[1].start));
}

/// A loop that no cut cures is collapsed, not stored by the thousand.
#[test]
fn a_loop_that_survives_every_cut_is_collapsed() {
    let x = audio(700.0, &[]);
    let e = Engine::new(0.0); // everything loops
    let (t, report) = run(&e, &x, None, &mut |_| true).unwrap();
    assert!(report.repeats_dropped > 0, "{report:?}");
    assert!(t.words.len() < 200, "{} words", t.words.len());
    assert!(find_runs(&t.words).is_empty());
}

#[test]
fn short_audio_runs_once_and_progress_never_goes_back() {
    let x = audio(300.0, &[]);
    let e = Engine::new(1e9);
    let mut seen = Vec::new();
    let (_, report) = run(&e, &x, None, &mut |p| {
        seen.push(p);
        true
    })
    .unwrap();
    assert_eq!(report.pieces, 1);
    assert_eq!(e.calls.lock().unwrap().len(), 1);
    assert!(seen.windows(2).all(|w| w[0] <= w[1]), "{seen:?}");
    assert_eq!(seen.last(), Some(&1.0), "done is reported once, at the end");
    assert!(seen[..seen.len() - 1].iter().all(|p| *p < 1.0));

    // With splits (restarts of the progress inside) it still only goes forward.
    let x = audio(1500.0, &[]);
    let e = Engine::new(100.0);
    let mut seen = Vec::new();
    run(&e, &x, None, &mut |p| {
        seen.push(p);
        true
    })
    .unwrap();
    assert!(seen.windows(2).all(|w| w[0] <= w[1]));
}

#[test]
fn a_cancel_stops_the_run_and_stays_a_cancel() {
    let x = audio(1500.0, &[]);
    let e = Engine::new(1e9);
    let mut n = 0;
    let err = run(&e, &x, None, &mut |_| {
        n += 1;
        n < 6
    })
    .unwrap_err();
    assert!(crate::transcribe::is_cancelled(&err), "{err:?}");
    let mut e = Engine::new(1e9);
    e.cancel_after = Some(1);
    let err = run(&e, &x, None, &mut |_| true).unwrap_err();
    assert!(crate::transcribe::is_cancelled(&err));
}

#[test]
fn a_silent_recording_keeps_the_engines_note() {
    struct Silent;
    impl Transcriber for Silent {
        fn transcribe(
            &self,
            _: Pcm16k<'_>,
            _: Option<&str>,
            _: &mut dyn FnMut(f32) -> bool,
        ) -> Result<Transcript, CmError> {
            Ok(Transcript {
                version: TRANSCRIPT_VERSION,
                language: "en".into(),
                model: "m".into(),
                words: vec![],
                note: Some("No speech detected".into()),
            })
        }
    }
    let x = audio(5.0, &[]);
    let audio = Pcm16k {
        samples: &x,
        start: MediaTime::ZERO,
    };
    let (t, _) = transcribe_pieces(&Silent, audio, None, None, &mut |_| true).unwrap();
    assert!(t.words.is_empty());
    assert_eq!(t.note.as_deref(), Some("No speech detected"));
}
