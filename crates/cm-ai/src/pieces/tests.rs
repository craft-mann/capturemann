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

/// Seen on Oct 28, 2026: an 88 minute transcription at 7x% started over from 0% when a recording
/// held the job queue. A checkpoint after every finished piece lets it go on instead.
#[test]
fn a_run_goes_on_from_its_checkpoint_and_gives_the_same_transcript() {
    let x = audio(1500.0, &[]);
    let whole = Engine::new(1e9);
    let (expected, _) = run(&whole, &x, None, &mut |_| true).unwrap();
    let pieces = whole.calls.lock().unwrap().len();
    assert!(pieces >= 4);

    // Run until the third piece has started, keeping the checkpoints, then stop (the hold).
    let first = Engine::new(1e9);
    let mut checkpoints: Vec<Checkpoint> = Vec::new();
    let mut started = 0;
    fn audio_of(x: &[f32]) -> Pcm16k<'_> {
        Pcm16k {
            samples: x,
            start: MediaTime::from_ticks(1200),
        }
    }
    let err = transcribe_pieces_resumable(
        &first,
        audio_of(&x),
        None,
        Some("glossary: Linh, Minh"),
        None,
        &mut |c| checkpoints.push(c.clone()),
        None,
        &mut |p| {
            // Every piece reports 4 steps; stop in the third piece.
            started += 1;
            let _ = p;
            started < 10
        },
    )
    .unwrap_err();
    assert!(crate::transcribe::is_cancelled(&err));
    assert_eq!(checkpoints.len(), 2, "one checkpoint per finished piece");
    let last = checkpoints.last().unwrap().clone();
    assert_eq!((last.done, last.pieces), (2, pieces));
    assert_eq!(last.language.as_deref(), Some("vi"));

    // The checkpoint goes through JSON, as the job stores it.
    let stored: Checkpoint = serde_json::from_value(serde_json::to_value(&last).unwrap()).unwrap();
    let second = Engine::new(1e9);
    let mut seen = Vec::new();
    let mut later = Vec::new();
    let (t, _) = transcribe_pieces_resumable(
        &second,
        audio_of(&x),
        None,
        Some("glossary: Linh, Minh"),
        Some(stored),
        &mut |c| later.push(c.done),
        None,
        &mut |p| {
            seen.push(p);
            true
        },
    )
    .unwrap();
    // Only the pieces after the checkpoint were run, with the language already known.
    let calls = second.calls.lock().unwrap();
    assert_eq!(calls.len(), pieces - 2);
    assert!(calls.iter().all(|c| c.1.as_deref() == Some("vi")));
    assert_eq!(later, (3..pieces).collect::<Vec<_>>());
    // The transcript is the one an uninterrupted run gives.
    assert_eq!(t.words, expected.words);
    assert_eq!(t.language, expected.language);
    // The bar starts where the finished pieces end, not at 0.
    assert!(seen[0] > 0.3, "{seen:?}");
    assert!(seen.windows(2).all(|w| w[0] <= w[1]));
    assert_eq!(seen.last(), Some(&1.0));
}

/// A checkpoint of other audio is not used: the run starts over, and the words are right.
#[test]
fn a_checkpoint_that_does_not_fit_is_ignored() {
    let x = audio(1500.0, &[]);
    let whole = Engine::new(1e9);
    let (expected, _) = run(&whole, &x, None, &mut |_| true).unwrap();
    let pieces = whole.calls.lock().unwrap().len();
    let stale = Checkpoint {
        total_samples: x.len() + 1,
        pieces,
        done: 2,
        words: vec![word("stale", 1.0)],
        language: Some("en".into()),
        first: None,
        split: 0,
        repeats_dropped: 0,
        other_audio: 0,
        other_audio_kept: 0,
    };
    let e = Engine::new(1e9);
    let audio = Pcm16k {
        samples: &x,
        start: MediaTime::from_ticks(1200),
    };
    let (t, _) = transcribe_pieces_resumable(
        &e,
        audio,
        None,
        None,
        Some(stale),
        &mut |_| {},
        None,
        &mut |_| true,
    )
    .unwrap();
    assert_eq!(e.calls.lock().unwrap().len(), pieces, "every piece was run");
    assert!(t.words.iter().all(|w| w.text != "stale"));
    assert_eq!(t.words.len(), expected.words.len());
}

/// Short audio is one piece: nothing is checkpointed (there is nothing to go on from).
#[test]
fn one_piece_makes_no_checkpoint() {
    let x = audio(300.0, &[]);
    let e = Engine::new(1e9);
    let mut n = 0;
    let audio = Pcm16k {
        samples: &x,
        start: MediaTime::ZERO,
    };
    transcribe_pieces_resumable(
        &e,
        audio,
        None,
        None,
        None,
        &mut |_| n += 1,
        None,
        &mut |_| true,
    )
    .unwrap();
    assert_eq!(n, 0);
}

// ---- the checks: soft loops, and the second reading from the other audio ------

/// A sentence of 14 words said over and over with one word changed each time: not an exact loop,
/// the way the noise-reduced reading of the long recording looped.
fn soft_loop(times: usize, from: f64) -> Vec<Word> {
    let variants = ["data", "quality", "model"];
    let mut out = Vec::new();
    for i in 0..times {
        // The number is where the loop is, so two loops in a transcript are not each other's repeat.
        let sentence = format!(
            "thì nó sẽ là cái {} của nó {} và mình sẽ nói chuyện với bên business",
            variants[i % 3],
            from as usize
        );
        for w in sentence.split_whitespace() {
            // Every word carries where the loop is: no group of words repeats across two loops.
            let tagged = format!("{w}{}", from as usize);
            out.push(word(&tagged, from + out.len() as f64 * 0.4));
        }
    }
    out
}

fn real_talk(n: usize, from: f64) -> Vec<Word> {
    // Every word is its own (the number says where), as real talk is: no group repeats.
    let at = (from * 100.0) as usize;
    (0..n)
        .map(|i| word(&format!("lời{}", at + i), from + i as f64 * 0.4))
        .collect()
}

#[test]
fn words_are_folded_to_letters_without_marks() {
    assert_eq!(fold("Ghiền"), "ghien");
    assert_eq!(fold("Đã,"), "da");
    assert_eq!(fold("Mì"), fold("mi"));
    assert_eq!(fold("Gõ?"), "go");
    assert_eq!(fold("1.5"), "15");
}

/// The soft loop of Oct 28, 2026: the exact check sees nothing, the repetition check does.
#[test]
fn a_soft_loop_is_found_where_the_exact_check_sees_none() {
    let looped = soft_loop(40, 0.0); // 40 x 14 = 560 words
    assert!(find_runs(&looped).is_empty(), "no exact loop in it");
    assert!(
        soft_loop_words(&looped) >= 300,
        "{}",
        soft_loop_words(&looped)
    );
    let b = Badness::of(&looped);
    assert!(b.is_bad() && !b.degenerate, "{b:?}");

    // Real talk with a few sentences said twice is not a loop (the original microphone had
    // stretches of 64 to 94 words like that).
    let mut talk = real_talk(300, 0.0);
    talk.extend(soft_loop(2, 200.0));
    talk.extend(real_talk(300, 300.0));
    assert!(
        soft_loop_words(&talk) < SOFT_LOOP_WORDS,
        "{}",
        soft_loop_words(&talk)
    );
    assert!(!Badness::of(&talk).is_bad());

    // A loop inside a longer piece is a region of its own.
    let mut piece = real_talk(500, 0.0);
    piece.extend(soft_loop(30, 250.0));
    piece.extend(real_talk(500, 500.0));
    let regions = repeat_regions(&piece);
    assert_eq!(regions.len(), 1, "{regions:?}");
    // The loop is words 500 to 1010 (30 sentences of 17 words).
    assert!(
        regions[0].start >= 500 && regions[0].end <= 1030,
        "{regions:?}"
    );
}

#[test]
fn dropping_soft_repeats_keeps_each_group_once() {
    let mut w = real_talk(200, 0.0);
    w.extend(soft_loop(30, 100.0));
    w.extend(real_talk(200, 300.0));
    let before = w.len();
    let (kept, dropped) = drop_soft_repeats(w);
    assert!(dropped > 200, "dropped {dropped}");
    assert_eq!(kept.len(), before - dropped);
    assert!(soft_loop_words(&kept) < SOFT_LOOP_WORDS);
    // The real talk before and after is all still there.
    assert_eq!(
        kept.iter().filter(|w| w.text.starts_with("lời")).count(),
        400
    );
}

/// What the scripted engine writes for a piece.
#[derive(Clone, Copy, PartialEq)]
enum Reading {
    Clean,
    SoftLoop,
}

/// An engine that tells the two audios apart: the main one is a loud tone, the other a quiet one.
struct TwoAudios {
    main: Reading,
    other: Reading,
    /// `(from the main audio, seconds)` of every call.
    calls: Mutex<Vec<(bool, f64)>>,
}

impl TwoAudios {
    fn new(main: Reading, other: Reading) -> TwoAudios {
        TwoAudios {
            main,
            other,
            calls: Mutex::new(Vec::new()),
        }
    }
}

impl Transcriber for TwoAudios {
    fn transcribe(
        &self,
        audio: Pcm16k<'_>,
        _: Option<&str>,
        progress: &mut dyn FnMut(f32) -> bool,
    ) -> Result<Transcript, CmError> {
        let secs = audio.samples.len() as f64 / RATE as f64;
        let is_main = audio.samples.iter().take(4000).any(|s| s.abs() > 0.2);
        self.calls.lock().unwrap().push((is_main, secs));
        if !progress(1.0) {
            return Err(cancelled());
        }
        let start = audio.start.as_secs_f64();
        let mut words = real_talk((secs / 4.0) as usize, start);
        match if is_main { self.main } else { self.other } {
            Reading::Clean => {}
            Reading::SoftLoop => words = soft_loop((secs / 5.0) as usize, start),
        }
        Ok(Transcript {
            version: TRANSCRIPT_VERSION,
            language: "vi".into(),
            model: "scripted".into(),
            words,
            note: None,
        })
    }
}

fn quiet_copy(x: &[f32]) -> Vec<f32> {
    x.iter().map(|s| s * 0.3).collect()
}

fn run_two(e: &TwoAudios, x: &[f32], other: Option<Vec<f32>>) -> (Transcript, PiecesReport, usize) {
    let mut asked = 0;
    let mut provider = || {
        asked += 1;
        other.clone()
    };
    let audio = Pcm16k {
        samples: x,
        start: MediaTime::ZERO,
    };
    let (t, report) = transcribe_pieces_resumable(
        e,
        audio,
        None,
        None,
        None,
        &mut |_| {},
        Some(&mut provider),
        &mut |_| true,
    )
    .unwrap();
    (t, report, asked)
}

/// The noise-reduced reading loops in a piece, the original does not: the piece is read again from
/// the other audio and the clean reading is kept, without cutting the piece up.
#[test]
fn a_looping_piece_is_read_again_from_the_other_audio() {
    let x = audio(900.0, &[]); // 3 pieces of 5 minutes
    let e = TwoAudios::new(Reading::SoftLoop, Reading::Clean);
    let (t, report, asked) = run_two(&e, &x, Some(quiet_copy(&x)));
    assert_eq!(asked, 1, "the other audio is made once, when first needed");
    assert!(
        report.other_audio >= 3 && report.other_audio_kept == report.other_audio,
        "{report:?}"
    );
    assert_eq!(report.split, 0, "no cut was needed");
    assert!(soft_loop_words(&t.words) < SOFT_LOOP_WORDS);
    assert!(
        t.words.iter().all(|w| w.text.starts_with("lời")),
        "only the clean reading"
    );
    let calls = e.calls.lock().unwrap();
    assert_eq!(
        calls.iter().filter(|c| c.0).count(),
        calls.iter().filter(|c| !c.0).count()
    );
}

/// A clean piece never makes the other audio: nothing is decoded or read twice.
#[test]
fn a_clean_recording_does_not_touch_the_other_audio() {
    let x = audio(900.0, &[]);
    let e = TwoAudios::new(Reading::Clean, Reading::SoftLoop);
    let (_, report, asked) = run_two(&e, &x, Some(quiet_copy(&x)));
    assert_eq!(asked, 0);
    assert_eq!(report.other_audio, 0);
    assert!(e.calls.lock().unwrap().iter().all(|c| c.0));
}

/// Bad in both: the piece is cut in half, and the repeats that survive are taken out.
#[test]
fn what_both_readings_get_wrong_is_cut_up_and_cleaned() {
    let x = audio(400.0, &[]);
    let e = TwoAudios::new(Reading::SoftLoop, Reading::SoftLoop);
    let (t, report, _) = run_two(&e, &x, Some(quiet_copy(&x)));
    assert!(report.split > 0 && report.other_audio > 0, "{report:?}");
    // 400 s of loop is about 1,300 words a reading; what is left is the first sentences of each
    // loop, not the loops.
    assert!(t.words.len() < 400, "{} words", t.words.len());
    assert!(report.repeats_dropped > 800, "{report:?}");
}

/// Another audio that is not the same length is not the same timeline: it is not used.
#[test]
fn another_audio_of_a_different_length_is_ignored() {
    let x = audio(400.0, &[]);
    let e = TwoAudios::new(Reading::SoftLoop, Reading::Clean);
    let short = quiet_copy(&x[..x.len() - 20 * RATE]);
    let (_, report, asked) = run_two(&e, &x, Some(short));
    assert_eq!(asked, 1);
    assert_eq!(report.other_audio, 0, "nothing was read from it");
    assert!(e.calls.lock().unwrap().iter().all(|c| c.0));
    // No other audio at all: the same, and no crash.
    let e = TwoAudios::new(Reading::SoftLoop, Reading::Clean);
    let (_, report, _) = run_two(&e, &x, None);
    assert_eq!(report.other_audio, 0);
}
