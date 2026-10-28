//! Making the notes of a report with a text model (AI-9), one call for a short recording and
//! map-reduce for a long one (AI-7 chunking: notes per chunk, then merge passes, always with the
//! same schema). Every reply is checked against the schema and then against the transcript
//! ([`super::check`]), so what comes out cites the recording and names real people.

use std::cell::RefCell;

use serde_json::{json, Map, Value};

use super::check::{self, Checker};
use super::prompt;
use super::{Line, MarkerIn, ReportContext, Source, Template};
use crate::text_model::chunk::{estimate_tokens, map_reduce, split_chunks, CHARS_PER_TOKEN};
use crate::text_model::{schema, ErrorKind, JsonRequest, Message, TextModel, TextModelError};

/// Everything a run needs.
pub struct Input<'a> {
    pub template: &'a Template,
    /// The caption lines (the edited ones when the user edited them), with speakers.
    pub lines: &'a [Line],
    /// Markers in session seconds.
    pub markers: &'a [MarkerIn],
    pub context: &'a ReportContext,
    /// Speaker names (AI-8), empty without speaker labels.
    pub speakers: &'a [String],
    /// Output language code; `None` is the transcript's.
    pub language: Option<&'a str>,
    pub source_language: &'a str,
    /// The model's context size in tokens (`ai.text.context_tokens`).
    pub context_tokens: usize,
}

/// What a run produced.
#[derive(Debug, Clone, PartialEq)]
pub struct Generated {
    /// `{ <section id>: [item, ...] }` for every section of the template, markers and context
    /// sections included.
    pub content: Value,
    pub dropped_uncited: usize,
    pub reassigned_names: usize,
    pub dropped_names: usize,
    /// Pieces the transcript was cut into (1 for a short recording).
    pub chunks: usize,
    /// Calls made to the model.
    pub model_calls: usize,
}

/// Why a run ended without a report.
#[derive(Debug, Clone, PartialEq)]
pub enum Error {
    Cancelled,
    Model(TextModelError),
}

impl From<TextModelError> for Error {
    fn from(e: TextModelError) -> Self {
        Error::Model(e)
    }
}

fn names(input: &Input<'_>) -> Vec<String> {
    let mut v: Vec<String> = input.context.attendees.clone();
    v.extend(input.speakers.iter().cloned());
    v
}

/// About how many tokens one call writes (a meeting's notes are about 1,000): inside a call the
/// bar is at `n / (n + EXPECTED_TOKENS)` of the call's share after `n` streamed tokens, so half
/// of it is passed at about this many and it never stops or overruns.
pub const EXPECTED_TOKENS: f32 = 700.0;

/// The caller's progress hook, made monotonic: whatever the calls report, the bar never goes
/// back, and it stays under 1.0 until [`generate`] ends.
struct Reporter<'a> {
    f: &'a mut dyn FnMut(f32) -> bool,
    high: f32,
}

impl Reporter<'_> {
    fn report(&mut self, p: f32) -> bool {
        self.high = self.high.max(p.clamp(0.0, 0.999));
        (self.f)(self.high)
    }
}

type Progress<'a, 'b> = &'a RefCell<Reporter<'b>>;

#[derive(Default)]
struct Totals {
    dropped_uncited: usize,
    reassigned_names: usize,
    dropped_names: usize,
    calls: usize,
}

/// One call. `slice` is the part of the whole bar this call may use; the tokens streamed from the
/// model move the bar inside it.
#[allow(clippy::too_many_arguments)]
fn ask(
    model: &dyn TextModel,
    input: &Input<'_>,
    system: &str,
    user: String,
    checker: &Checker,
    totals: &RefCell<Totals>,
    progress: Progress<'_, '_>,
    slice: (f32, f32),
) -> Result<Value, Error> {
    let max_tokens = (input.context_tokens / 3).clamp(1024, 4096) as u32;
    let req = JsonRequest {
        system: Some(system.to_owned()),
        messages: vec![Message::user(user)],
        schema: input.template.schema.clone(),
        schema_name: format!("report_{}", input.template.id),
        max_tokens,
    };
    let reply = model
        .complete_json_streaming(&req, &mut |n| {
            let n = n as f32;
            let frac = n / (n + EXPECTED_TOKENS);
            progress
                .borrow_mut()
                .report(slice.0 + (slice.1 - slice.0) * frac)
        })
        .map_err(|e| {
            if e.kind == ErrorKind::Cancelled {
                Error::Cancelled
            } else {
                Error::Model(e)
            }
        })?;
    if let Err(e) = schema::validate(&input.template.schema, &reply.value) {
        return Err(Error::Model(TextModelError {
            kind: ErrorKind::InvalidOutput,
            message: format!("The text model's notes do not match the report schema: {e}"),
        }));
    }
    let checked = check::apply(input.template, &reply.value, checker);
    let mut t = totals.borrow_mut();
    t.calls += 1;
    t.dropped_uncited += checked.dropped_uncited;
    t.reassigned_names += checked.reassigned_names;
    t.dropped_names += checked.dropped_names;
    Ok(checked.content)
}

/// A call whose notes did not fit in one reply (`output_too_long`).
fn too_long(e: &Error) -> bool {
    matches!(e, Error::Model(m) if m.kind == ErrorKind::OutputTooLong)
}

/// Puts checked notes side by side: every section is a list of items, so the union of the lists is
/// a valid set of notes (without the merge pass's de-duplication). Used when the model cannot write
/// the merged notes in one reply, so no call is needed and nothing can run over.
fn concat_notes(parts: &[Value]) -> Value {
    let mut out = Map::new();
    for p in parts {
        let Some(o) = p.as_object() else { continue };
        for (k, v) in o {
            match (out.get_mut(k), v) {
                (Some(Value::Array(a)), Value::Array(b)) => a.extend(b.iter().cloned()),
                (None, _) => {
                    out.insert(k.clone(), v.clone());
                }
                _ => {}
            }
        }
    }
    Value::Object(out)
}

/// Cuts `text` in two at the line break nearest its middle (or at a character if it has none).
fn halve(text: &str) -> (&str, &str) {
    let mid = text.len() / 2;
    let at = text[..mid]
        .rfind('\n')
        .or_else(|| text[mid..].find('\n').map(|i| mid + i))
        .map(|i| i + 1)
        .filter(|&i| i > 0 && i < text.len())
        .unwrap_or_else(|| {
            let mut i = mid;
            while !text.is_char_boundary(i) {
                i += 1;
            }
            i
        });
    text.split_at(at)
}

/// The notes of one piece of the transcript. When the model cannot write them in one reply, the
/// piece is cut in two and each half is done on its own (up to three levels, an eighth of the
/// piece), and the two sets of notes are put side by side.
#[allow(clippy::too_many_arguments)]
fn notes_for(
    model: &dyn TextModel,
    input: &Input<'_>,
    system: &str,
    preamble: &str,
    chunk: &str,
    part: Option<(usize, usize)>,
    checker: &Checker,
    totals: &RefCell<Totals>,
    progress: Progress<'_, '_>,
    slice: (f32, f32),
    depth: usize,
) -> Result<Value, Error> {
    let user = prompt::notes_prompt(preamble, chunk, part);
    match ask(model, input, system, user, checker, totals, progress, slice) {
        Err(e) if too_long(&e) && depth < 3 && chunk.chars().count() > 400 => {
            let (a, b) = halve(chunk);
            let mid = (slice.0 + slice.1) / 2.0;
            let first = notes_for(
                model,
                input,
                system,
                preamble,
                a,
                part,
                checker,
                totals,
                progress,
                (slice.0, mid),
                depth + 1,
            )?;
            let second = notes_for(
                model,
                input,
                system,
                preamble,
                b,
                part,
                checker,
                totals,
                progress,
                (mid, slice.1),
                depth + 1,
            )?;
            Ok(concat_notes(&[first, second]))
        }
        r => r,
    }
}

/// Merges `notes` (each already checked) into one set, in groups that fit a chunk of the context,
/// level by level, until one is left.
#[allow(clippy::too_many_arguments)]
fn merge_all(
    model: &dyn TextModel,
    input: &Input<'_>,
    system: &str,
    preamble: &str,
    mut level: Vec<Value>,
    checker: &Checker,
    totals: &RefCell<Totals>,
    progress: Progress<'_, '_>,
    base: f32,
) -> Result<Value, Error> {
    let budget_chars = (input.context_tokens as f64 * 0.4 * CHARS_PER_TOKEN) as usize;
    let overhead = system.chars().count() + preamble.chars().count() + 600;
    let room = budget_chars.saturating_sub(overhead).max(1000);
    let mut round = 0usize;
    while level.len() > 1 {
        round += 1;
        let mut next = Vec::new();
        let mut group: Vec<Value> = Vec::new();
        let mut size = 0usize;
        let mut groups: Vec<Vec<Value>> = Vec::new();
        for n in level {
            let len = n.to_string().chars().count();
            if group.len() >= 2 && size + len > room {
                groups.push(std::mem::take(&mut group));
                size = 0;
            }
            size += len;
            group.push(n);
        }
        if !group.is_empty() {
            groups.push(group);
        }
        let total = groups.len();
        for (i, g) in groups.into_iter().enumerate() {
            // Each round halves what is left, so the bar only moves forward.
            let before = 0.5f32.powi(round as i32 - 1);
            let after = 0.5f32.powi(round as i32);
            let at = |k: usize| {
                let done = 1.0 - before + (before - after) * (k as f32 / total as f32);
                base + (0.99 - base) * done
            };
            if !progress.borrow_mut().report(at(i)) {
                return Err(Error::Cancelled);
            }
            if g.len() == 1 {
                next.extend(g);
                continue;
            }
            let user = prompt::merge_prompt(preamble, &g);
            let slice = (at(i), at(i + 1));
            match ask(model, input, system, user, checker, totals, progress, slice) {
                Ok(v) => next.push(v),
                // The merged notes do not fit in one reply: keep all of them side by side.
                Err(e) if too_long(&e) => next.push(concat_notes(&g)),
                Err(e) => return Err(e),
            }
        }
        level = next;
    }
    Ok(level.pop().unwrap_or_else(|| json!({})))
}

/// Runs the template over the transcript. `progress` gets 0.0 to 1.0 and returns `false` to stop
/// ([`Error::Cancelled`]).
pub fn generate(
    model: &dyn TextModel,
    input: &Input<'_>,
    progress: &mut dyn FnMut(f32) -> bool,
) -> Result<Generated, Error> {
    let text = prompt::transcript_text(input.lines);
    let system = prompt::system_prompt(input.template, input.language, input.source_language);
    let preamble = prompt::preamble(input.template, input.context, input.speakers, input.markers);
    let checker = Checker::new(input.lines, &names(input));
    let n_chunks = split_chunks(&text, input.context_tokens).len().max(1);
    let totals = RefCell::new(Totals::default());
    let progress = RefCell::new(Reporter {
        f: progress,
        high: 0.0,
    });
    let progress = &progress;
    let map_share = if n_chunks > 1 { 0.7 } else { 0.95 };

    let notes = map_reduce(
        &text,
        input.context_tokens,
        |i, chunk| {
            let slice = (
                map_share * i as f32 / n_chunks as f32,
                map_share * (i + 1) as f32 / n_chunks as f32,
            );
            if !progress.borrow_mut().report(slice.0) {
                return Err(Error::Cancelled);
            }
            let part = (n_chunks > 1).then_some((i + 1, n_chunks));
            notes_for(
                model, input, &system, &preamble, chunk, part, &checker, &totals, progress, slice,
                0,
            )
        },
        |parts| {
            if parts.len() <= 1 {
                return Ok(parts.into_iter().next().unwrap_or_else(|| json!({})));
            }
            merge_all(
                model, input, &system, &preamble, parts, &checker, &totals, progress, map_share,
            )
        },
    )?;
    let model_notes = if notes.is_object() { notes } else { json!({}) };
    let content = assemble(input, &model_notes);
    let t = totals.into_inner();
    let _ = (progress.borrow_mut().f)(1.0);
    Ok(Generated {
        content,
        dropped_uncited: t.dropped_uncited,
        reassigned_names: t.reassigned_names,
        dropped_names: t.dropped_names,
        chunks: n_chunks,
        model_calls: t.calls,
    })
}

/// The stored content: the checked model sections, in the template's order, plus the sections
/// made from the markers and from the context.
pub fn assemble(input: &Input<'_>, model_notes: &Value) -> Value {
    let mut out = Map::new();
    for s in &input.template.sections {
        match s.source {
            Source::Model => {
                let items = model_notes.get(&s.id).cloned().unwrap_or_else(|| json!([]));
                out.insert(s.id.clone(), items);
            }
            Source::Markers => {
                let mut ms: Vec<&MarkerIn> = input.markers.iter().collect();
                ms.sort_by(|a, b| a.at.total_cmp(&b.at));
                let items: Vec<Value> = ms
                    .iter()
                    .map(|m| {
                        json!({
                            "label": m.label.clone().filter(|l| !l.trim().is_empty())
                                .unwrap_or_else(|| "Marker".to_owned()),
                            "at": [(m.at * 10.0).round() / 10.0],
                        })
                    })
                    .collect();
                out.insert(s.id.clone(), Value::Array(items));
            }
            Source::Context => {
                let role = input
                    .context
                    .role
                    .clone()
                    .unwrap_or_else(|| "Not given".to_owned());
                out.insert(
                    s.id.clone(),
                    json!([{ "role": role, "people": input.context.attendees }]),
                );
            }
        }
    }
    Value::Object(out)
}

/// The tokens the transcript alone takes (for the Report panel's long-recording hint).
pub fn transcript_tokens(lines: &[Line]) -> usize {
    estimate_tokens(&prompt::transcript_text(lines))
}
