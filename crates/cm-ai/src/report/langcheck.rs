//! The language check of a report (AI-9.AC10): after the notes pass the transcript checks, the
//! free text of each section is checked for being in the output language, by the same text model,
//! as a yes or no question. No word lists, no per-language rules and no script heuristics: the
//! model that wrote the notes says whether a section reads as the language that was asked for.
//!
//! * One short call asks about all the sections at once (`{ parts: [{ id, in_language }] }`), so a
//!   report that passes costs one extra call.
//! * A section that fails is written again once, from its own items with a stronger instruction
//!   (translate every free-text field; keep the citations, names, dates and quotes), one call per
//!   failing section. The second version replaces the first only when it has the same number of
//!   items and a second batched check of the rewritten sections passes.
//! * A section that still fails keeps its first version and is counted in `language_unsure`.
//!
//! A check call that fails for a reason other than a stop leaves the notes as they are and counts
//! nothing: no verdict is not a verdict against the notes.

use serde_json::{json, Map, Value};

use super::check::{self, Checker};
use super::generate::{Error, Input};
use super::prompt;
use super::{FieldKind, Section};
use crate::text_model::{ErrorKind, JsonRequest, Message, TextModel};

/// The most characters of one section the check reads.
const MAX_CHECK_CHARS: usize = 1200;

/// What the pass did.
#[derive(Debug, Clone, PartialEq)]
pub struct Outcome {
    pub notes: Value,
    /// Sections that were not in the output language and still are not.
    pub unsure: usize,
    /// Sections that were written again and replaced the first version.
    pub rewritten: usize,
    /// Calls made to the model.
    pub calls: usize,
}

/// The free text of a section as one block of lines: every prose field of every item (a list
/// field one line per entry). Person fields, quotes, dates and the like (`prose: false`) are left
/// out. Empty when there is nothing to judge.
pub fn prose_text(section: &Section, items: &[Value]) -> String {
    let mut out = String::new();
    for item in items {
        for f in section
            .fields
            .iter()
            .filter(|f| f.prose && f.person.is_none())
        {
            match f.kind {
                FieldKind::Text => {
                    if let Some(s) = item.get(&f.id).and_then(Value::as_str) {
                        push_line(&mut out, s);
                    }
                }
                FieldKind::List => {
                    for s in item
                        .get(&f.id)
                        .and_then(Value::as_array)
                        .into_iter()
                        .flatten()
                        .filter_map(Value::as_str)
                    {
                        push_line(&mut out, s);
                    }
                }
            }
        }
    }
    out
}

fn push_line(out: &mut String, s: &str) {
    let s = s.split_whitespace().collect::<Vec<_>>().join(" ");
    if !s.is_empty() {
        out.push_str(&s);
        out.push('\n');
    }
}

fn clip(s: &str) -> &str {
    match s.char_indices().nth(MAX_CHECK_CHARS) {
        Some((i, _)) => &s[..i],
        None => s,
    }
}

fn check_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "parts": {
                "type": "array",
                "items": {
                    "type": "object",
                    "properties": {
                        "id": { "type": "string", "maxLength": 80 },
                        "in_language": { "type": "boolean" }
                    },
                    "required": ["id", "in_language"],
                    "additionalProperties": false
                },
                "maxItems": 40
            }
        },
        "required": ["parts"],
        "additionalProperties": false
    })
}

fn check_system(language: &str) -> String {
    format!(
        "You decide which language a text is written in. Answer with one JSON object that matches \
         the given schema and nothing else. For each part, `in_language` is true when its sentences \
         are written in {language}. It stays true when people's names, product names, technical \
         terms, numbers or a few quoted words are in another language. It is false when the \
         sentences themselves are written in another language, or when they mix sentences of \
         another language into the text."
    )
}

fn check_user(language: &str, parts: &[(&str, String)]) -> String {
    let mut s = format!("Is the text of each part below written in {language}?\n");
    for (id, text) in parts {
        s.push_str(&format!("\n### {id}\n{}\n", clip(text)));
    }
    s
}

/// One batched question. `Ok(Some(ids))`: the parts that are not in the language; `Ok(None)`: no
/// verdict (the call failed or its answer did not fit the schema).
fn ask_language(
    model: &dyn TextModel,
    language: &str,
    parts: &[(&str, String)],
    calls: &mut usize,
) -> Result<Option<Vec<String>>, Error> {
    let req = JsonRequest {
        system: Some(check_system(language)),
        messages: vec![Message::user(check_user(language, parts))],
        schema: check_schema(),
        schema_name: "report_language_check".to_owned(),
        max_tokens: 512,
    };
    *calls += 1;
    let reply = match model.complete_json(&req) {
        Ok(r) => r,
        Err(e) if e.kind == ErrorKind::Cancelled => return Err(Error::Cancelled),
        Err(e) => {
            log::warn!("report language check: no verdict: {}", e.message);
            return Ok(None);
        }
    };
    if crate::text_model::schema::validate(&req.schema, &reply.value).is_err() {
        return Ok(None);
    }
    let wrong = reply
        .value
        .get("parts")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter(|p| p.get("in_language") == Some(&json!(false)))
        .filter_map(|p| p.get("id").and_then(Value::as_str))
        .filter(|id| parts.iter().any(|(pid, _)| pid == id))
        .map(str::to_owned)
        .collect();
    Ok(Some(wrong))
}

/// The template schema reduced to one section.
fn section_schema(schema: &Value, id: &str) -> Value {
    let mut s = schema.clone();
    if let Some(p) = s.get_mut("properties").and_then(Value::as_object_mut) {
        p.retain(|k, _| k == id);
    }
    if let Some(r) = s.get_mut("required").and_then(Value::as_array_mut) {
        r.retain(|k| k.as_str() == Some(id));
    }
    s
}

/// Writes one section again in `language`, from its own items. `None` when the call fails, the
/// answer does not fit the schema, or it does not have the same number of items after the
/// transcript checks.
#[allow(clippy::too_many_arguments)]
fn rewrite(
    model: &dyn TextModel,
    input: &Input<'_>,
    system: &str,
    language: &str,
    section: &Section,
    items: &Value,
    checker: &Checker,
    calls: &mut usize,
) -> Result<Option<Value>, Error> {
    let user = format!(
        "The notes of the section \"{title}\" below were written in the wrong language. Write them \
         again completely in {language}: translate every free-text field, keep the same items in \
         the same order, and keep every `at` list and every name, owner, date and quote exactly as \
         it is. Answer with the JSON object that has only the key `{id}`.\n\n{items}\n\n\
         Reminder: every sentence must be in {language}. Answer with the JSON object only.\n",
        title = section.title,
        id = section.id,
    );
    let req = JsonRequest {
        system: Some(format!(
            "{system}\nThis is a correction: the previous attempt wrote some of the text in a \
             language other than {language}. This time write it all in {language}."
        )),
        messages: vec![Message::user(user)],
        schema: section_schema(&input.template.schema, &section.id),
        schema_name: format!("report_{}_{}", input.template.id, section.id),
        max_tokens: (input.context_tokens / 3).clamp(1024, 4096) as u32,
    };
    *calls += 1;
    let reply = match model.complete_json(&req) {
        Ok(r) => r,
        Err(e) if e.kind == ErrorKind::Cancelled => return Err(Error::Cancelled),
        Err(e) => {
            log::warn!("report language rewrite failed: {}", e.message);
            return Ok(None);
        }
    };
    if crate::text_model::schema::validate(&req.schema, &reply.value).is_err() {
        return Ok(None);
    }
    let checked = check::apply(input.template, &reply.value, checker);
    let new = checked.content.get(&section.id).cloned();
    let same = |a: &Value, b: &Value| a.as_array().map(Vec::len) == b.as_array().map(Vec::len);
    Ok(new.filter(|n| same(n, items)))
}

/// Runs the check over `notes` (the checked model sections). `keep_going` is called before every
/// model call and returns `false` to stop ([`Error::Cancelled`]).
pub fn run(
    model: &dyn TextModel,
    input: &Input<'_>,
    system: &str,
    notes: &Value,
    checker: &Checker,
    keep_going: &mut dyn FnMut() -> bool,
) -> Result<Outcome, Error> {
    let mut out = Outcome {
        notes: notes.clone(),
        unsure: 0,
        rewritten: 0,
        calls: 0,
    };
    let Some(language) = prompt::output_language(input.language, input.source_language) else {
        return Ok(out);
    };
    let texts: Vec<(&Section, String)> = input
        .template
        .model_sections()
        .filter_map(|s| {
            let items = notes.get(&s.id).and_then(Value::as_array)?;
            let t = prose_text(s, items);
            (!t.trim().is_empty()).then_some((s, t))
        })
        .collect();
    if texts.is_empty() {
        return Ok(out);
    }
    if !keep_going() {
        return Err(Error::Cancelled);
    }
    let parts: Vec<(&str, String)> = texts
        .iter()
        .map(|(s, t)| (s.id.as_str(), t.clone()))
        .collect();
    let Some(wrong) = ask_language(model, &language, &parts, &mut out.calls)? else {
        return Ok(out);
    };
    if wrong.is_empty() {
        return Ok(out);
    }

    // Each failing section once more, from its own items.
    let mut candidates: Vec<(&Section, Value)> = Vec::new();
    for (section, _) in texts.iter().filter(|(s, _)| wrong.contains(&s.id)) {
        if !keep_going() {
            return Err(Error::Cancelled);
        }
        let items = notes.get(&section.id).cloned().unwrap_or_else(|| json!([]));
        if let Some(new) = rewrite(
            model,
            input,
            system,
            &language,
            section,
            &items,
            checker,
            &mut out.calls,
        )? {
            candidates.push((section, new));
        }
    }

    // Keep a rewrite only when it passes the same question.
    let mut accepted: Vec<&str> = Vec::new();
    if !candidates.is_empty() {
        if !keep_going() {
            return Err(Error::Cancelled);
        }
        let parts: Vec<(&str, String)> = candidates
            .iter()
            .map(|(s, items)| {
                (
                    s.id.as_str(),
                    prose_text(s, items.as_array().map_or(&[][..], Vec::as_slice)),
                )
            })
            .collect();
        if let Some(still_wrong) = ask_language(model, &language, &parts, &mut out.calls)? {
            accepted = candidates
                .iter()
                .map(|(s, _)| s.id.as_str())
                .filter(|id| !still_wrong.iter().any(|w| w == id))
                .collect();
        }
    }
    let mut notes = out.notes.as_object().cloned().unwrap_or_else(Map::new);
    for (section, new) in candidates {
        if accepted.contains(&section.id.as_str()) {
            notes.insert(section.id.clone(), new);
            out.rewritten += 1;
        }
    }
    out.unsure = wrong.len() - out.rewritten;
    out.notes = Value::Object(notes);
    Ok(out)
}
