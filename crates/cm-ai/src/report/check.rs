//! What a report may contain (AI-9): every item cites the recording, and names come from the
//! conversation.
//!
//! * A citation (`at`, seconds) is **kept** when it falls inside a transcript line (within
//!   [`CITE_TOLERANCE_S`](super::CITE_TOLERANCE_S) of its edges) and **removed** otherwise. An item
//!   left without any citation is dropped and counted in `dropped_uncited` (AI-9.AC3). The
//!   attendee list is the one place where an item may have none (someone named in the context who
//!   never spoke).
//! * An owner that is not one of the attendees or speakers and does not appear as words of the
//!   transcript becomes "Unassigned"; an attendee with such a name is dropped.

use std::collections::HashSet;

use serde_json::{json, Map, Value};

use super::{Line, Person, Section, Template, CITE_TOLERANCE_S};
use crate::textmatch::{is_word_char, nfc};

/// The word an owner nobody can name becomes.
pub const UNASSIGNED: &str = "Unassigned";

/// The facts a report is checked against.
pub struct Checker {
    /// Sorted, merged `(start, end)` of the transcript lines, in seconds.
    intervals: Vec<(f64, f64)>,
    /// Attendees and speaker names, normalized.
    names: HashSet<String>,
    /// Every word of the transcript, normalized.
    words: HashSet<String>,
}

fn norm(s: &str) -> String {
    nfc(s).to_lowercase()
}

fn tokens(s: &str) -> Vec<String> {
    norm(s)
        .split(|c: char| !is_word_char(c))
        .filter(|w| !w.is_empty())
        .map(str::to_owned)
        .collect()
}

impl Checker {
    /// `names`: the attendees the user typed and the speaker names.
    pub fn new(lines: &[Line], names: &[String]) -> Checker {
        let mut intervals: Vec<(f64, f64)> = lines
            .iter()
            .filter(|l| !l.text.trim().is_empty())
            .map(|l| (l.start, l.end.max(l.start)))
            .collect();
        intervals.sort_by(|a, b| a.0.total_cmp(&b.0));
        let mut merged: Vec<(f64, f64)> = Vec::new();
        for (s, e) in intervals {
            match merged.last_mut() {
                Some(last) if s <= last.1 + 2.0 * CITE_TOLERANCE_S => last.1 = last.1.max(e),
                _ => merged.push((s, e)),
            }
        }
        let mut words = HashSet::new();
        for l in lines {
            words.extend(tokens(&l.text));
        }
        Checker {
            intervals: merged,
            names: names.iter().map(|n| norm(n.trim())).collect(),
            words,
        }
    }

    /// Whether `t` falls inside a transcript line.
    pub fn cited(&self, t: f64) -> bool {
        if !t.is_finite() || t < 0.0 {
            return false;
        }
        // The merged intervals are sorted and apart: the last one starting at or before t+tol.
        let i = self
            .intervals
            .partition_point(|(s, _)| *s <= t + CITE_TOLERANCE_S);
        i > 0 && t <= self.intervals[i - 1].1 + CITE_TOLERANCE_S
    }

    /// Whether `name` may be shown as an owner or attendee.
    pub fn name_ok(&self, name: &str) -> bool {
        let n = norm(name.trim());
        if n.is_empty() {
            return false;
        }
        if n == norm(UNASSIGNED) || self.names.contains(&n) {
            return true;
        }
        // "Linh" for the attendee "Linh Tran", or "Linh Tran" said as two words.
        let parts = tokens(name);
        if parts.is_empty() {
            return false;
        }
        let listed = self
            .names
            .iter()
            .any(|full| parts.iter().all(|p| tokens(full).contains(p)));
        listed || parts.iter().all(|p| self.words.contains(p))
    }
}

/// The result of checking one set of notes.
#[derive(Debug, Clone, PartialEq)]
pub struct Checked {
    pub content: Value,
    /// Items dropped because none of their citations matched the transcript.
    pub dropped_uncited: usize,
    /// Owners replaced by "Unassigned".
    pub reassigned_names: usize,
    /// Attendees dropped because the name is nobody's.
    pub dropped_names: usize,
}

fn at_values(item: &Map<String, Value>) -> Vec<f64> {
    item.get("at")
        .and_then(Value::as_array)
        .map(|a| a.iter().filter_map(Value::as_f64).collect())
        .unwrap_or_default()
}

/// Round to a tenth of a second and drop repeats, so `at` reads `[723.4]`.
fn tidy(mut v: Vec<f64>) -> Vec<f64> {
    v = v.into_iter().map(|t| (t * 10.0).round() / 10.0).collect();
    v.sort_by(f64::total_cmp);
    v.dedup();
    v
}

fn check_section(
    section: &Section,
    items: &[Value],
    checker: &Checker,
    out: &mut Checked,
) -> Vec<Value> {
    let mut kept = Vec::new();
    'items: for item in items {
        let Some(obj) = item.as_object() else {
            continue;
        };
        let cites = tidy(
            at_values(obj)
                .into_iter()
                .filter(|t| checker.cited(*t))
                .collect(),
        );
        if cites.is_empty() && !section.at_optional {
            out.dropped_uncited += 1;
            continue;
        }
        let mut clean = Map::new();
        for f in &section.fields {
            let v = obj.get(&f.id);
            match f.kind {
                super::FieldKind::List => {
                    let list: Vec<String> = v
                        .and_then(Value::as_array)
                        .map(|a| {
                            a.iter()
                                .filter_map(Value::as_str)
                                .map(|s| s.trim().to_owned())
                                .filter(|s| !s.is_empty())
                                .collect()
                        })
                        .unwrap_or_default();
                    clean.insert(f.id.clone(), json!(list));
                }
                super::FieldKind::Text => {
                    let mut text = v
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                        .trim()
                        .to_owned();
                    match f.person {
                        Some(Person::Owner) => {
                            if text.is_empty() || !checker.name_ok(&text) {
                                if !text.is_empty() {
                                    out.reassigned_names += 1;
                                }
                                text = UNASSIGNED.to_owned();
                            }
                        }
                        Some(Person::Attendee) if !checker.name_ok(&text) || text == UNASSIGNED => {
                            out.dropped_names += 1;
                            continue 'items;
                        }
                        Some(Person::Attendee) => {}
                        None => {}
                    }
                    clean.insert(f.id.clone(), json!(text));
                }
            }
        }
        // An item without its first field has nothing to say.
        let first_empty = section.fields.first().is_some_and(|f| match f.kind {
            super::FieldKind::Text => clean
                .get(&f.id)
                .and_then(Value::as_str)
                .is_none_or(str::is_empty),
            super::FieldKind::List => clean
                .get(&f.id)
                .and_then(Value::as_array)
                .is_none_or(Vec::is_empty),
        });
        if first_empty {
            continue;
        }
        clean.insert("at".into(), json!(cites));
        kept.push(Value::Object(clean));
    }
    kept
}

/// Checks the notes of the model sections of `template`. Sections that are missing become empty;
/// anything that is not a section of the template is ignored.
pub fn apply(template: &Template, notes: &Value, checker: &Checker) -> Checked {
    let mut out = Checked {
        content: Value::Null,
        dropped_uncited: 0,
        reassigned_names: 0,
        dropped_names: 0,
    };
    let mut content = Map::new();
    for section in template.model_sections() {
        let items: &[Value] = notes
            .get(&section.id)
            .and_then(Value::as_array)
            .map_or(&[], Vec::as_slice);
        let mut kept = check_section(section, items, checker, &mut out);
        if section.max_items > 0 {
            kept.truncate(section.max_items);
        }
        content.insert(section.id.clone(), Value::Array(kept));
    }
    out.content = Value::Object(content);
    out
}
