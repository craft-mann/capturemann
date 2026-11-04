//! The normalisation run: turns in batches to the text model, every answer checked by the
//! word-preserving invariant, one stricter retry per failed turn, then the raw text.

use cm_core::CmError;
use serde::{Deserialize, Serialize};
use serde_json::json;

use super::invariant::{check_turn, word_key, Violation};
use super::prompt;
use super::turns::{self, Span};
use crate::text_model::{chunk, ErrorKind, JsonRequest, TextModel, TextModelError};
use crate::transcribe::Word;

/// The result for one piece of the transcript.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TurnResult {
    /// First word (index into `transcript.json`) and number of words.
    pub first: usize,
    pub len: usize,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub speaker: Option<u32>,
    /// The model's answer passed the invariant. `false`: the raw words stand.
    pub normalized: bool,
    /// Why it was rejected: `changed_words`, `dropped_words`, `added_words`, `too_long`,
    /// `no_answer`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    /// One string per word of the piece (the raw word when `normalized` is false).
    pub display: Vec<String>,
    /// Words (relative to `first`) that continue the word before without a space.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub glued: Vec<usize>,
    /// Words (relative to `first`) that start a paragraph.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub breaks: Vec<usize>,
}

/// What the run did, for the job result and the benchmark.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Stats {
    pub model_calls: usize,
    /// Pieces whose first answer failed the invariant and were asked again.
    pub retried: usize,
    /// Failures of the first answers by kind (before any retry).
    pub first_substituted: usize,
    pub first_deleted: usize,
    pub first_inserted: usize,
    /// Pieces split in two after failing the check twice.
    pub bisected: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Options {
    pub language: String,
    pub context_tokens: usize,
    /// Most words in one call.
    pub batch_words: usize,
    /// Most turns in one call.
    pub batch_turns: usize,
}

impl Options {
    pub fn new(language: &str, context_tokens: usize) -> Options {
        Options {
            language: language.to_owned(),
            context_tokens,
            batch_words: 120,
            batch_turns: 12,
        }
    }
}

#[derive(Debug)]
pub enum NormalizeError {
    Cancelled,
    Model(TextModelError),
}

impl NormalizeError {
    pub fn into_cm_error(self, job_id: &str) -> CmError {
        match self {
            NormalizeError::Cancelled => crate::transcribe::cancelled(),
            NormalizeError::Model(e) => e.into_cm_error(job_id),
        }
    }
}

impl From<TextModelError> for NormalizeError {
    fn from(e: TextModelError) -> Self {
        if e.kind == ErrorKind::Cancelled {
            NormalizeError::Cancelled
        } else {
            NormalizeError::Model(e)
        }
    }
}

/// How many words of the turn before are shown as context.
const CONTEXT_WORDS: usize = 30;
/// A piece that failed twice is split in two while it has at least this many words.
const BISECT_MIN_WORDS: usize = 12;

struct Piece<'a> {
    span: Span,
    /// Positions (relative to the span) of the words that have a key.
    keyed: Vec<usize>,
    keys: Vec<String>,
    text: String,
    words: &'a [Word],
}

impl<'a> Piece<'a> {
    fn new(words: &'a [Word], span: Span) -> Piece<'a> {
        let mut keyed = Vec::new();
        let mut keys = Vec::new();
        let mut parts: Vec<&str> = Vec::new();
        for (k, w) in words[span.first..span.end()].iter().enumerate() {
            let key = word_key(&w.text);
            if !key.is_empty() {
                keyed.push(k);
                keys.push(key);
                parts.push(w.text.trim());
            }
        }
        Piece {
            span,
            keyed,
            keys,
            text: parts.join(" "),
            words,
        }
    }

    fn raw(&self) -> Vec<String> {
        self.words[self.span.first..self.span.end()]
            .iter()
            .map(|w| w.text.trim().to_owned())
            .collect()
    }

    fn rejected(&self, reason: &str) -> TurnResult {
        TurnResult {
            first: self.span.first,
            len: self.span.len,
            speaker: self.span.speaker,
            normalized: false,
            reason: Some(reason.to_owned()),
            display: self.raw(),
            glued: Vec::new(),
            breaks: Vec::new(),
        }
    }

    /// Checks `answer`; the result of a pass, or the violation.
    fn accept(&self, answer: &str) -> Result<TurnResult, Violation> {
        let a = check_turn(&self.keys, answer)?;
        let mut display = self.raw();
        for (k, d) in a.display.into_iter().enumerate() {
            display[self.keyed[k]] = d;
        }
        Ok(TurnResult {
            first: self.span.first,
            len: self.span.len,
            speaker: self.span.speaker,
            normalized: true,
            reason: None,
            display,
            glued: a.glued.iter().map(|&k| self.keyed[k]).collect(),
            breaks: a.breaks.iter().map(|&k| self.keyed[k]).collect(),
        })
    }
}

struct Run<'a, 'p> {
    model: &'a dyn TextModel,
    words: &'a [Word],
    opts: &'a Options,
    progress: &'p mut dyn FnMut(f32) -> bool,
    total_words: f32,
    done_words: usize,
    stats: Stats,
}

impl Run<'_, '_> {
    fn ask(
        &mut self,
        items: &[String],
        before: &str,
        strict: Option<&str>,
    ) -> Result<Vec<String>, NormalizeError> {
        let words: usize = items.iter().map(|i| i.split_whitespace().count()).sum();
        let schema = json!({
            "type": "object",
            "properties": { "turns": { "type": "array", "items": { "type": "string" },
                                        "minItems": items.len(), "maxItems": items.len() } },
            "required": ["turns"],
            "additionalProperties": false
        });
        let mut jr = JsonRequest::new(
            prompt::user(&self.opts.language, items, before, strict),
            schema,
        );
        jr.system = Some(prompt::system());
        jr.schema_name = "normalized_turns".to_owned();
        jr.max_tokens = (words as u32 * 5 + 96).clamp(256, 4096);
        self.stats.model_calls += 1;
        let frac = self.done_words as f32 / self.total_words.max(1.0);
        let progress = &mut *self.progress;
        let reply = self
            .model
            .complete_json_streaming(&jr, &mut |_| progress(frac.min(0.999)))?;
        let arr = reply.value["turns"].as_array().ok_or_else(|| {
            NormalizeError::Model(TextModelError {
                kind: ErrorKind::InvalidOutput,
                message: "the text model reply has no `turns`".to_owned(),
            })
        })?;
        Ok(arr
            .iter()
            .map(|v| v.as_str().unwrap_or_default().to_owned())
            .collect())
    }

    fn count(&mut self, v: &Violation) {
        self.stats.first_substituted += v.substituted;
        self.stats.first_deleted += v.deleted;
        self.stats.first_inserted += v.inserted;
    }

    /// One piece on its own with the stricter prompt (`first` is the violation of the earlier
    /// answer, if there was one). A reply that does not fit halves the piece.
    fn single(
        &mut self,
        piece: &Piece<'_>,
        before: &str,
        earlier: Option<&Violation>,
        out: &mut Vec<TurnResult>,
    ) -> Result<(), NormalizeError> {
        if piece.keys.is_empty() {
            out.push(
                piece
                    .accept("")
                    .unwrap_or_else(|_| piece.rejected("no_answer")),
            );
            return Ok(());
        }
        let why = match earlier {
            Some(v) => match v.reason() {
                "dropped_words" => "words were left out",
                "added_words" => "words were added",
                _ => "words were changed",
            },
            None => "the answer did not have the right shape",
        };
        let answer = match self.ask(std::slice::from_ref(&piece.text), before, Some(why)) {
            Ok(mut a) if a.len() == 1 => a.remove(0),
            Ok(_) => {
                out.push(piece.rejected("no_answer"));
                return Ok(());
            }
            Err(NormalizeError::Model(e))
                if matches!(e.kind, ErrorKind::OutputTooLong | ErrorKind::InvalidOutput) =>
            {
                if e.kind == ErrorKind::OutputTooLong {
                    if let Some((a, b)) = turns::halve(self.words, piece.span, 8) {
                        let pa = Piece::new(self.words, a);
                        let pb = Piece::new(self.words, b);
                        self.single(&pa, before, None, out)?;
                        let tail = tail_of(out.last());
                        self.single(&pb, &tail, None, out)?;
                        return Ok(());
                    }
                    out.push(piece.rejected("too_long"));
                } else {
                    out.push(piece.rejected("no_answer"));
                }
                return Ok(());
            }
            Err(e) => return Err(e),
        };
        match piece.accept(&answer) {
            Ok(r) => out.push(r),
            Err(v) => {
                if earlier.is_none() {
                    self.count(&v);
                }
                // Isolate the damage: each half is asked on its own, so one bad word keeps a
                // few words raw instead of the whole turn. The halves are checked like any
                // other piece; nothing that fails the check is ever used.
                if let Some((a, b)) = turns::halve(self.words, piece.span, BISECT_MIN_WORDS) {
                    self.stats.bisected += 1;
                    let pa = Piece::new(self.words, a);
                    let pb = Piece::new(self.words, b);
                    self.single(&pa, before, Some(&v), out)?;
                    let tail = tail_of(out.last());
                    self.single(&pb, &tail, Some(&v), out)?;
                } else {
                    out.push(piece.rejected(v.reason()));
                }
            }
        }
        Ok(())
    }
}

/// The last words of a result, as context for the next piece.
fn tail_of(r: Option<&TurnResult>) -> String {
    let Some(r) = r else {
        return String::new();
    };
    let from = r.display.len().saturating_sub(CONTEXT_WORDS);
    r.display[from..].join(" ")
}

/// Normalises `words`. `resume` holds the results of an earlier, interrupted run over the same
/// words (they must tile the start of the transcript, else they are dropped).
/// `on_checkpoint` gets all results so far after each batch; `progress` gets 0.0 to 1.0 and
/// returns `false` to cancel.
pub fn normalize(
    model: &dyn TextModel,
    words: &[Word],
    speakers: Option<&[Option<u32>]>,
    opts: &Options,
    resume: Vec<TurnResult>,
    on_checkpoint: &mut dyn FnMut(&[TurnResult]),
    progress: &mut dyn FnMut(f32) -> bool,
) -> Result<(Vec<TurnResult>, Stats), NormalizeError> {
    let spans = turns::build(words, speakers);
    let mut results: Vec<TurnResult> = Vec::new();
    let mut at = 0usize;
    for r in resume {
        if r.first == at && r.len > 0 && r.display.len() == r.len && at + r.len <= words.len() {
            at += r.len;
            results.push(r);
        } else {
            break;
        }
    }
    let mut pending: Vec<Span> = spans.into_iter().filter(|s| s.first >= at).collect();
    pending.reverse(); // a stack: the next span is last
    if pending.last().is_some_and(|s| s.first != at) {
        // The saved results do not end on a turn boundary of this run: start over.
        results.clear();
        at = 0;
        pending = turns::build(words, speakers);
        pending.reverse();
    }
    let max_chars =
        (opts.context_tokens as f64 * chunk::CHUNK_SHARE * chunk::CHARS_PER_TOKEN) as usize;
    let mut run = Run {
        model,
        words,
        opts,
        progress,
        total_words: words.len() as f32,
        done_words: at,
        stats: Stats::default(),
    };
    let mut batch_cap = opts.batch_turns.max(1);
    while !pending.is_empty() {
        if !(run.progress)(run.done_words as f32 / run.total_words.max(1.0)) {
            return Err(NormalizeError::Cancelled);
        }
        let mut batch: Vec<Piece<'_>> = Vec::new();
        let (mut nwords, mut nchars) = (0usize, 0usize);
        while let Some(&s) = pending.last() {
            let p = Piece::new(words, s);
            let c = p.text.chars().count();
            let w = p.keys.len();
            if !batch.is_empty()
                && (batch.len() >= batch_cap
                    || nwords + w > opts.batch_words
                    || nchars + c > max_chars)
            {
                break;
            }
            nwords += w;
            nchars += c;
            pending.pop();
            batch.push(p);
        }
        let before = tail_of(results.last());
        let mut out: Vec<TurnResult> = Vec::with_capacity(batch.len());
        let ask_items: Vec<usize> = (0..batch.len())
            .filter(|&i| !batch[i].keys.is_empty())
            .collect();
        let items: Vec<String> = ask_items.iter().map(|&i| batch[i].text.clone()).collect();
        let answers = if items.is_empty() {
            Ok(Vec::new())
        } else {
            run.ask(&items, &before, None)
        };
        match answers {
            Err(NormalizeError::Model(e)) if e.kind == ErrorKind::OutputTooLong => {
                if batch.len() > 1 {
                    // Ask for less: the same turns go back, in smaller batches.
                    batch_cap = (batch.len() / 2).max(1);
                    for p in batch.iter().rev() {
                        pending.push(p.span);
                    }
                    continue;
                }
                let p = &batch[0];
                run.single(p, &before, None, &mut out)?;
            }
            Err(NormalizeError::Model(e)) if e.kind == ErrorKind::InvalidOutput => {
                // The shape was wrong (or the model could not be made to answer): each turn alone.
                let mut ctx = before.clone();
                for p in &batch {
                    run.single(p, &ctx, None, &mut out)?;
                    ctx = tail_of(out.last());
                }
            }
            Err(e) => return Err(e),
            Ok(a) if a.len() != items.len() => {
                run.stats.retried += batch.len();
                let mut ctx = before.clone();
                for p in &batch {
                    run.single(p, &ctx, None, &mut out)?;
                    ctx = tail_of(out.last());
                }
            }
            Ok(a) => {
                let mut ai = 0usize;
                let mut ctx = before.clone();
                for p in &batch {
                    if p.keys.is_empty() {
                        out.push(p.accept("").unwrap_or_else(|_| p.rejected("no_answer")));
                        continue;
                    }
                    match p.accept(&a[ai]) {
                        Ok(r) => out.push(r),
                        Err(v) => {
                            run.count(&v);
                            run.stats.retried += 1;
                            run.single(p, &ctx, Some(&v), &mut out)?;
                        }
                    }
                    ai += 1;
                    ctx = tail_of(out.last());
                }
            }
        }
        for r in &out {
            run.done_words += r.len;
        }
        results.extend(out);
        on_checkpoint(&results);
    }
    let _ = (run.progress)(1.0);
    Ok((results, run.stats))
}
