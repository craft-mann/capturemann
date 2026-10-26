//! The whisper.cpp implementation of [`Transcriber`] (through the `whisper-rs` binding, built
//! from the vendored whisper.cpp source; Metal on macOS, CPU elsewhere).
//!
//! Words come from the model's token timestamps: a token that starts with a space begins a word,
//! the following tokens without one continue it (sub-word pieces, punctuation). Non-speech
//! annotations such as `[BLANK_AUDIO]` or `(music)` are dropped. When a segment's token times are
//! unusable (all zero, backwards), its words share the segment's span in proportion to their
//! length. [`build_words`] is pure and tested without a model.

use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicI32, Ordering};
use std::sync::Arc;
use std::time::Duration;

use cm_core::{CmError, MediaTime};
use whisper_rs::{
    DtwMode, DtwModelPreset, DtwParameters, FullParams, SamplingStrategy, WhisperContext,
    WhisperContextParameters,
};

use crate::transcribe::{cancelled, Pcm16k, Transcriber, Transcript, Word, TRANSCRIPT_VERSION};

/// One model token as the binding reports it. Times are in 10 ms units from the audio start.
#[derive(Debug, Clone, PartialEq)]
pub struct RawToken {
    pub text: String,
    pub t0: i64,
    pub t1: i64,
    pub p: f32,
    /// The token's DTW alignment time (10 ms units; roughly where the token ends in the audio),
    /// `-1` when the model ran without DTW. It does not depend on where the segment starts, which
    /// the default token times do (they put the first words of a clip that opens with silence at
    /// 0.0).
    pub dtw: i64,
    /// A control token (`[_BEG_]`, `<|endoftext|>`, timestamp tokens).
    pub special: bool,
}

/// One decoded segment: its span and tokens. Times are in 10 ms units.
#[derive(Debug, Clone, PartialEq)]
pub struct RawSegment {
    pub t0: i64,
    pub t1: i64,
    pub tokens: Vec<RawToken>,
}

const TICKS_PER_CENTISECOND: i64 = cm_core::TICKS_PER_SECOND / 100;

/// Groups segments' tokens into timed words, offset by `base` (the time of the first sample).
pub fn build_words(segments: &[RawSegment], base: MediaTime) -> Vec<Word> {
    let mut out: Vec<Word> = Vec::new();
    // End of the last word so far, in 10 ms units: a word never starts before it.
    let mut prev_end_cs = 0i64;
    for seg in segments {
        // (text, t0, t1, probability sum, token count, DTW end or -1)
        let mut words: Vec<(String, i64, i64, f32, u32, i64)> = Vec::new();
        for t in seg.tokens.iter().filter(|t| !t.special) {
            if t.text.is_empty() {
                continue;
            }
            let starts_word = t.text.starts_with([' ', '\n', '\t']) || words.is_empty();
            let piece = t.text.trim_start_matches([' ', '\n', '\t']);
            if piece.is_empty() {
                continue;
            }
            if starts_word {
                words.push((piece.to_owned(), t.t0, t.t1, t.p, 1, t.dtw));
            } else if let Some(w) = words.last_mut() {
                w.0.push_str(piece);
                w.2 = t.t1;
                w.3 += t.p;
                w.4 += 1;
                w.5 = if w.5 < 0 || t.dtw < 0 { -1 } else { t.dtw };
            }
        }
        words.retain(|w| !is_annotation(&w.0));
        if words.is_empty() {
            continue;
        }
        let monotonic = words.windows(2).all(|p| p[1].1 >= p[0].1);
        let usable =
            monotonic && words.iter().all(|w| w.2 >= w.1) && words.iter().any(|w| w.2 > w.1);
        let dtw_ok = words.iter().all(|w| w.5 >= 0) && words.windows(2).all(|p| p[1].5 >= p[0].5);
        let spans: Vec<(i64, i64)> = if dtw_ok {
            dtw_spans(&words, prev_end_cs)
        } else if usable {
            words
                .iter()
                .map(|w| (w.1.clamp(seg.t0, seg.t1), w.2.clamp(seg.t0, seg.t1)))
                .collect()
        } else {
            // Share the segment's span by text length.
            let weights: Vec<i64> = words
                .iter()
                .map(|w| w.0.chars().count().max(1) as i64)
                .collect();
            let total: i64 = weights.iter().sum();
            let span = (seg.t1 - seg.t0).max(0);
            let mut at = seg.t0;
            let mut acc = 0;
            weights
                .iter()
                .map(|wt| {
                    let from = at;
                    acc += wt;
                    at = seg.t0 + span * acc / total;
                    (from, at)
                })
                .collect()
        };
        for (w, (a, b)) in words.into_iter().zip(spans) {
            let b = b.max(a);
            prev_end_cs = b;
            out.push(Word {
                text: w.0,
                start: base + MediaTime::from_ticks(a * TICKS_PER_CENTISECOND),
                end: base + MediaTime::from_ticks(b * TICKS_PER_CENTISECOND),
                confidence: (w.3 / w.4 as f32).clamp(0.0, 1.0),
            });
        }
    }
    out
}

/// Word spans from the tokens' DTW times. A token's DTW time is about where it ends, so a word
/// ends at its last token's time and starts where the word before it ended, but no more than a
/// plausible spoken length earlier (about 90 ms a letter, 0.15 to 1.2 s): after a silence a word
/// does not stretch back over it.
fn dtw_spans(words: &[(String, i64, i64, f32, u32, i64)], mut prev_end: i64) -> Vec<(i64, i64)> {
    words
        .iter()
        .map(|w| {
            let end = w.5.max(prev_end);
            let longest = (w.0.chars().count() as i64 * 9).clamp(15, 120);
            let start = (end - longest).max(prev_end);
            prev_end = end;
            (start, end)
        })
        .collect()
}

/// `[BLANK_AUDIO]`, `[MUSIC]`, `(applause)`, `*sigh*`: sound descriptions, not speech.
fn is_annotation(text: &str) -> bool {
    let t = text.trim();
    let wrapped = |open: char, close: char| {
        t.starts_with(open) && t.ends_with(close) && t.chars().count() >= 2
    };
    wrapped('[', ']')
        || wrapped('(', ')')
        || (t.len() >= 3 && t.starts_with('*') && t.ends_with('*'))
}

/// A loaded Whisper model. Loading reads the whole model file, so build one per job (or keep it
/// while the same model is used) and share it by reference.
pub struct WhisperTranscriber {
    ctx: WhisperContext,
    model: String,
    threads: i32,
}

impl WhisperTranscriber {
    /// Loads the model file. `model_id` is recorded in the transcript. With `use_gpu` the GPU
    /// backend is tried first and the CPU is used if it cannot start.
    pub fn load(model_path: &Path, model_id: &str, use_gpu: bool) -> Result<Self, CmError> {
        whisper_rs::install_logging_hooks();
        // DTW token times need the model's alignment heads, known for the stock models only; any
        // other id (or a context that cannot start with DTW) runs without and uses the default
        // token times.
        let load = |gpu: bool, dtw: bool| {
            let mut params = WhisperContextParameters::default();
            params.use_gpu(gpu);
            if let Some(preset) = dtw.then(|| dtw_preset(model_id)).flatten() {
                params.dtw_parameters(DtwParameters {
                    mode: DtwMode::ModelPreset {
                        model_preset: preset,
                    },
                    dtw_mem_size: 128 * 1024 * 1024,
                });
            }
            WhisperContext::new_with_params(model_path, params)
        };
        let ctx = load(use_gpu, true)
            .or_else(|_| load(use_gpu, false))
            .or_else(|e| if use_gpu { load(false, false) } else { Err(e) })
            .map_err(|e| bad_model(model_path, &e))?;
        let threads = std::thread::available_parallelism()
            .map(|n| n.get().min(8))
            .unwrap_or(4) as i32;
        Ok(WhisperTranscriber {
            ctx,
            model: model_id.to_owned(),
            threads,
        })
    }
}

/// The alignment-head preset of a stock model id (`large` is large-v3).
fn dtw_preset(model_id: &str) -> Option<DtwModelPreset> {
    match model_id {
        "tiny" => Some(DtwModelPreset::Tiny),
        "base" => Some(DtwModelPreset::Base),
        "small" => Some(DtwModelPreset::Small),
        "medium" => Some(DtwModelPreset::Medium),
        "large" => Some(DtwModelPreset::LargeV3),
        _ => None,
    }
}

fn bad_model(path: &Path, e: &dyn std::fmt::Display) -> CmError {
    CmError::invalid_input(format!(
        "{} is not a usable Whisper model ({e}). Download it again from the AI page.",
        path.display()
    ))
}

/// What one run of the model produced.
struct RunOutput {
    segments: Vec<RawSegment>,
    language: String,
}

fn run(
    ctx: &WhisperContext,
    samples: &[f32],
    language: String,
    threads: i32,
    prompt: Option<&str>,
    progress: Arc<AtomicI32>,
    cancel: Arc<AtomicBool>,
) -> Result<RunOutput, CmError> {
    let internal = |what: &str, e: &dyn std::fmt::Display| {
        CmError::internal(format!("speech recognition {what}: {e}"))
    };
    let mut state = ctx
        .create_state()
        .map_err(|e| internal("could not start", &e))?;
    let mut params = FullParams::new(SamplingStrategy::Greedy { best_of: 1 });
    params.set_n_threads(threads);
    params.set_translate(false);
    params.set_language(Some(language.as_str()));
    params.set_print_special(false);
    params.set_print_progress(false);
    params.set_print_realtime(false);
    params.set_print_timestamps(false);
    params.set_token_timestamps(true);
    params.set_suppress_blank(true);
    params.set_suppress_nst(true);
    if let Some(p) = prompt {
        params.set_initial_prompt(p);
    }
    params.set_progress_callback_safe(move |p| progress.store(p, Ordering::Relaxed));
    // Not `set_abort_callback_safe`: whisper-rs 0.16 stores a pointer to a boxed trait object but
    // its callback reads it back as the concrete closure, so the "abort?" answer is garbage and
    // every run fails with whisper error -6 ("failed to encode"). This callback reads the flag
    // directly; `cancel` outlives `full`, which is the only place the pointer is used.
    // SAFETY: whisper.cpp calls this only with the user-data pointer registered below, which is
    // a live `Arc<AtomicBool>` pointer for as long as `full` runs.
    unsafe extern "C" fn abort_requested(data: *mut std::ffi::c_void) -> bool {
        // A panic must not unwind into whisper.cpp. If one happens the state is unknown, so answer
        // "abort": the run ends with an error instead of continuing blind.
        cm_core::guard::guard("whisper abort_requested", true, || {
            // SAFETY: `data` is `Arc::as_ptr` of the `cancel` flag below, alive for the whole call.
            unsafe { &*(data as *const AtomicBool) }.load(Ordering::Relaxed)
        })
    }
    // SAFETY: the callback and its data pointer stay valid until `full` returns (see above).
    unsafe {
        params.set_abort_callback(Some(abort_requested));
        params.set_abort_callback_user_data(Arc::as_ptr(&cancel) as *mut std::ffi::c_void);
    }
    let full = state.full(params, samples);
    drop(cancel);
    full.map_err(|e| internal("failed", &e))?;

    let mut segments = Vec::new();
    for seg in state.as_iter() {
        let mut pieces = Vec::new();
        for i in 0..seg.n_tokens() {
            let Some(tok) = seg.get_token(i) else {
                continue;
            };
            // Raw bytes, not text: a token can end in the middle of a multi-byte character.
            let bytes = tok.to_bytes().map(<[u8]>::to_vec).unwrap_or_default();
            let d = tok.token_data();
            pieces.push(TokenBytes {
                bytes,
                t0: d.t0,
                t1: d.t1,
                p: d.p,
                dtw: d.t_dtw,
            });
        }
        let tokens = join_split_characters(pieces);
        segments.push(RawSegment {
            t0: seg.start_timestamp(),
            t1: seg.end_timestamp(),
            tokens,
        });
    }
    let language = whisper_rs::get_lang_str(state.full_lang_id_from_state())
        .unwrap_or("en")
        .to_owned();
    Ok(RunOutput { segments, language })
}

/// A token's bytes before they are decoded.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct TokenBytes {
    pub bytes: Vec<u8>,
    pub t0: i64,
    pub t1: i64,
    pub p: f32,
    pub dtw: i64,
}

fn is_control(text: &str) -> bool {
    text.starts_with("[_") || text.starts_with("<|")
}

/// Decodes tokens, joining the ones that split a multi-byte character between them.
///
/// Whisper's tokens are byte-level pieces, so a Vietnamese letter such as "ầ" (three bytes) can be
/// cut between two tokens. Decoding each token by itself turns both halves into U+FFFD ("c��n").
/// Bytes are collected until they form valid UTF-8; the joined token spans from the first start to
/// the last end and keeps the lowest probability (a doubtful half makes the whole letter doubtful).
/// A control token, or bytes that can never become valid, end a group and are decoded lossily.
pub(crate) fn join_split_characters(pieces: Vec<TokenBytes>) -> Vec<RawToken> {
    fn flush(group: &mut Option<TokenBytes>, out: &mut Vec<RawToken>) {
        if let Some(g) = group.take() {
            let text = String::from_utf8_lossy(&g.bytes).into_owned();
            let special = is_control(&text);
            out.push(RawToken {
                text,
                t0: g.t0,
                t1: g.t1,
                p: g.p,
                dtw: g.dtw,
                special,
            });
        }
    }
    let mut out = Vec::new();
    let mut group: Option<TokenBytes> = None;
    for piece in pieces {
        match group.as_mut() {
            // Only an unfinished character (not a control token) is continued.
            Some(g) if !is_control(&String::from_utf8_lossy(&piece.bytes)) => {
                g.bytes.extend_from_slice(&piece.bytes);
                g.t1 = piece.t1;
                g.p = g.p.min(piece.p);
                g.dtw = piece.dtw;
            }
            _ => {
                flush(&mut group, &mut out);
                group = Some(piece);
            }
        }
        // A group that is valid text, or can never become valid, is complete.
        let done = group
            .as_ref()
            .is_none_or(|g| match std::str::from_utf8(&g.bytes) {
                Ok(_) => true,
                Err(e) => e.error_len().is_some(),
            });
        if done {
            flush(&mut group, &mut out);
        }
    }
    flush(&mut group, &mut out);
    out
}

impl Transcriber for WhisperTranscriber {
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
        // whisper-rs panics on a NUL byte in the prompt.
        let prompt = prompt
            .map(|p| p.replace('\0', ""))
            .filter(|p| !p.trim().is_empty());
        let requested = match language.map(str::trim).filter(|l| !l.is_empty()) {
            None => "auto".to_owned(),
            Some(l) if l.eq_ignore_ascii_case("auto") => "auto".to_owned(),
            Some(l) => {
                let l = l.to_ascii_lowercase();
                if whisper_rs::get_lang_id(&l).is_none() {
                    return Err(CmError::invalid_field(
                        "language",
                        format!("`{l}` is not a language Whisper knows (use `auto` or a code such as `en`, `vi`, `es`)"),
                    ));
                }
                l
            }
        };
        // Whisper needs a little audio; shorter input has no speech worth reporting.
        if audio.samples.len() < 1600 {
            return Ok(Transcript {
                version: TRANSCRIPT_VERSION,
                language: if requested == "auto" {
                    "en".into()
                } else {
                    requested
                },
                model: self.model.clone(),
                words: Vec::new(),
                note: Some("No speech detected".to_owned()),
            });
        }
        let pct = Arc::new(AtomicI32::new(0));
        let cancel = Arc::new(AtomicBool::new(false));
        let samples = audio.samples;
        let outcome = std::thread::scope(|scope| {
            let (pct2, cancel2) = (pct.clone(), cancel.clone());
            let handle = scope.spawn(|| {
                run(
                    &self.ctx,
                    samples,
                    requested.clone(),
                    self.threads,
                    prompt.as_deref(),
                    pct2,
                    cancel2,
                )
            });
            while !handle.is_finished() {
                let p = pct.load(Ordering::Relaxed).clamp(0, 100);
                if !progress(p as f32 / 100.0) {
                    cancel.store(true, Ordering::Relaxed);
                }
                std::thread::sleep(Duration::from_millis(80));
            }
            handle.join()
        });
        if cancel.load(Ordering::Relaxed) {
            return Err(cancelled());
        }
        let out = outcome.map_err(|_| CmError::internal("speech recognition crashed"))??;
        let _ = progress(1.0);
        let words = build_words(&out.segments, audio.start);
        let note = words.is_empty().then(|| "No speech detected".to_owned());
        Ok(Transcript {
            version: TRANSCRIPT_VERSION,
            language: if requested == "auto" {
                out.language
            } else {
                requested
            },
            model: self.model.clone(),
            words,
            note,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn piece(bytes: &[u8], t0: i64, t1: i64, p: f32) -> TokenBytes {
        TokenBytes {
            dtw: -1,
            bytes: bytes.to_vec(),
            t0,
            t1,
            p,
        }
    }

    /// A character cut between two tokens is decoded whole (no U+FFFD), with the span of both and
    /// the lower probability.
    #[test]
    fn a_character_split_between_tokens_is_joined() {
        let a = "ầ".as_bytes(); // three bytes
        let out = join_split_characters(vec![
            piece(b" c", 0, 10, 0.9),
            piece(&a[..1], 10, 12, 0.8),
            piece(&a[1..], 12, 20, 0.5),
            piece(b"n", 20, 30, 0.7),
        ]);
        let texts: Vec<&str> = out.iter().map(|t| t.text.as_str()).collect();
        assert_eq!(texts, [" c", "ầ", "n"]);
        assert!(out.iter().all(|t| !t.text.contains('\u{FFFD}')));
        assert_eq!((out[1].t0, out[1].t1), (10, 20));
        assert_eq!(out[1].p, 0.5);
        // A cut after the first of two continuation bytes joins the same way.
        let out = join_split_characters(vec![piece(&a[..2], 0, 5, 0.6), piece(&a[2..], 5, 9, 0.9)]);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].text, "ầ");
    }

    /// Whole tokens pass through unchanged; a control token is never glued to a half character;
    /// bytes that can never be valid are decoded lossily instead of swallowing what follows.
    #[test]
    fn whole_tokens_control_tokens_and_bad_bytes_pass_through() {
        let out = join_split_characters(vec![
            piece("Xin".as_bytes(), 0, 4, 0.9),
            piece("<|endoftext|>".as_bytes(), 4, 4, 1.0),
            piece(&[0xE1, 0xBA], 4, 6, 0.4),
            piece("<|0.02|>".as_bytes(), 6, 6, 1.0),
            piece(&[0xFF], 6, 7, 0.4),
            piece(b"ok", 7, 9, 0.9),
        ]);
        assert_eq!(out[0].text, "Xin");
        assert!(!out[0].special);
        assert!(out[1].special);
        // The unfinished character before a control token is not joined with it.
        assert_eq!(out[2].text, "\u{FFFD}");
        assert!(out[3].special);
        assert_eq!(out[4].text, "\u{FFFD}");
        assert_eq!(out[5].text, "ok");
        assert_eq!(out.len(), 6);
    }

    fn tok(text: &str, t0: i64, t1: i64) -> RawToken {
        RawToken {
            dtw: -1,
            text: text.into(),
            t0,
            t1,
            p: 0.8,
            special: text.starts_with("[_"),
        }
    }

    fn secs(w: &Word) -> (f64, f64) {
        (w.start.as_secs_f64(), w.end.as_secs_f64())
    }

    fn dtw_tok(text: &str, dtw: i64) -> RawToken {
        RawToken {
            dtw,
            ..tok(text, 0, 0)
        }
    }

    /// With DTW times a clip that opens with 2 s of silence gets its first word near the sound, not
    /// at the segment start where the default token times put it.
    #[test]
    fn dtw_times_place_the_first_word_after_the_silence() {
        let seg = RawSegment {
            t0: 0,
            t1: 500,
            tokens: vec![
                tok("[_BEG_]", 0, 0),
                dtw_tok(" testing", 226),
                dtw_tok(" then", 252),
                dtw_tok(" record", 284),
                tok("[_TT_250]", 500, 500),
            ],
        };
        let words = build_words(&[seg], MediaTime::ZERO);
        let (a, b) = secs(&words[0]);
        assert_eq!(b, 2.26);
        // A plausible spoken length before its end, not stretched back to 0.
        assert!((1.5..2.2).contains(&a), "{a}");
        assert_eq!(secs(&words[1]), (2.26, 2.52));
        assert_eq!(secs(&words[2]), (2.52, 2.84));
    }

    /// After a pause a word does not stretch back over the silence.
    #[test]
    fn a_word_after_a_silence_keeps_a_spoken_length() {
        let seg = RawSegment {
            t0: 0,
            t1: 1000,
            tokens: vec![
                dtw_tok(" yes", 100),
                dtw_tok(" okay", 900),
                tok("[_TT_1]", 1000, 1000),
            ],
        };
        let words = build_words(&[seg], MediaTime::ZERO);
        let (a, b) = secs(&words[1]);
        assert_eq!(b, 9.0);
        assert!((8.2..8.8).contains(&a), "{a}");
    }

    /// Tokens without DTW times (a model run without it) keep the default times.
    #[test]
    fn without_dtw_the_default_token_times_are_used() {
        let seg = RawSegment {
            t0: 0,
            t1: 200,
            tokens: vec![tok(" hi", 10, 30), tok(" there", 40, 80)],
        };
        let words = build_words(&[seg], MediaTime::ZERO);
        assert_eq!(secs(&words[0]), (0.10, 0.30));
        assert_eq!(secs(&words[1]), (0.40, 0.80));
    }

    #[test]
    fn subword_tokens_and_punctuation_join_their_word() {
        let seg = RawSegment {
            t0: 0,
            t1: 200,
            tokens: vec![
                tok("[_BEG_]", 0, 0),
                tok(" Hel", 10, 30),
                tok("lo", 30, 60),
                tok(",", 60, 62),
                tok(" world", 70, 120),
                tok(".", 120, 125),
                tok("[_TT_300]", 200, 200),
            ],
        };
        let words = build_words(&[seg], MediaTime::ZERO);
        let text: Vec<&str> = words.iter().map(|w| w.text.as_str()).collect();
        assert_eq!(text, ["Hello,", "world."]);
        assert_eq!(secs(&words[0]), (0.10, 0.62));
        assert_eq!(secs(&words[1]), (0.70, 1.25));
        assert!((words[0].confidence - 0.8).abs() < 1e-6);
    }

    #[test]
    fn base_offsets_every_time() {
        let seg = RawSegment {
            t0: 0,
            t1: 100,
            tokens: vec![tok(" hi", 50, 100)],
        };
        let words = build_words(&[seg], MediaTime::from_secs(10));
        assert_eq!(secs(&words[0]), (10.5, 11.0));
    }

    #[test]
    fn annotations_are_dropped() {
        let seg = RawSegment {
            t0: 0,
            t1: 300,
            tokens: vec![
                tok(" [BLANK_AUDIO]", 0, 100),
                tok(" (music)", 100, 150),
                tok(" *sigh*", 150, 180),
                tok(" real", 200, 250),
            ],
        };
        let words = build_words(&[seg], MediaTime::ZERO);
        assert_eq!(words.len(), 1);
        assert_eq!(words[0].text, "real");
    }

    #[test]
    fn unusable_token_times_share_the_segment_span_by_length() {
        let seg = RawSegment {
            t0: 100,
            t1: 400,
            tokens: vec![tok(" aa", 0, 0), tok(" bbbbb", 0, 0), tok(" c", 0, 0)],
        };
        let words = build_words(&[seg], MediaTime::ZERO);
        assert_eq!(words.len(), 3);
        assert_eq!(secs(&words[0]).0, 1.0);
        assert_eq!(secs(&words[2]).1, 4.0);
        // Contiguous, ordered, and the longest word gets the most time.
        assert_eq!(words[0].end, words[1].start);
        assert_eq!(words[1].end, words[2].start);
        assert!(words[1].end - words[1].start > words[0].end - words[0].start);
    }

    #[test]
    fn empty_and_special_only_segments_give_no_words() {
        let seg = RawSegment {
            t0: 0,
            t1: 10,
            tokens: vec![tok("[_BEG_]", 0, 0)],
        };
        assert!(build_words(&[seg], MediaTime::ZERO).is_empty());
        assert!(build_words(&[], MediaTime::ZERO).is_empty());
    }

    #[test]
    fn a_segment_whose_first_token_has_no_space_still_starts_a_word() {
        let seg = RawSegment {
            t0: 0,
            t1: 100,
            tokens: vec![tok("Yes", 0, 50), tok(" sir", 50, 100)],
        };
        let words = build_words(&[seg], MediaTime::ZERO);
        assert_eq!(
            words.iter().map(|w| w.text.as_str()).collect::<Vec<_>>(),
            ["Yes", "sir"]
        );
    }
}
